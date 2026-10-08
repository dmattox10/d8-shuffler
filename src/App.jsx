import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Alert,
  Badge,
  Button,
  Checkbox,
  Field,
  FolderIcon,
  InputGroup,
  ListGroup,
  ListItem,
  Meter,
  Modal,
  NavBrand,
  NavSpacer,
  Navbar,
  Panel,
  RefreshIcon,
  Segmented,
  Toggle,
} from "./frostplate/index.jsx";
import { api, isMock } from "./api.js";

/* ---------------------------------------------------------------- helpers -- */

const gb = (n) => `${(n / 1e9).toFixed(n >= 1e10 ? 0 : 1)} GB`;

function duration(secs) {
  const s = Math.round(secs);
  const m = Math.floor(s / 60);
  return m ? `${m}m ${String(s % 60).padStart(2, "0")}s` : `${s}s`;
}

const saved = (key, fallback) => {
  try {
    const v = localStorage.getItem(`d8.${key}`);
    return v == null ? fallback : JSON.parse(v);
  } catch {
    return fallback;
  }
};
const save = (key, value) => {
  try {
    localStorage.setItem(`d8.${key}`, JSON.stringify(value));
  } catch {
    /* settings are a convenience, not a requirement */
  }
};

function useSetting(key, fallback) {
  const [value, setValue] = useState(() => saved(key, fallback));
  useEffect(() => save(key, value), [key, value]);
  return [value, setValue];
}

const PHASE_TEXT = {
  checking: "Checking everything before touching the card",
  wiping: "Clearing the card",
  copying: "Writing tracks",
  finishing: "Tidying up",
  done: "Finished",
};

function ShuffleIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor"
      strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <path d="M1.5 4.5h2.2c3.2 0 4.6 7 7.8 7h3" />
      <path d="M1.5 11.5h2.2c1.2 0 2.1-1 2.9-2.2M9.4 6.2c.6-.9 1.3-1.7 2.1-1.7h3" />
      <path d="M12.8 2.7 14.5 4.5l-1.7 1.8M12.8 9.7l1.7 1.8-1.7 1.8" />
    </svg>
  );
}

/* -------------------------------------------------------------------- app -- */

export default function App() {
  const [source, setSource] = useSetting("source", "");
  const [flacOk, setFlacOk] = useSetting("flacOk", false);
  const [quality, setQuality] = useSetting("quality", "0");
  const [eject, setEject] = useSetting("eject", true);
  const [fresh, setFresh] = useSetting("fresh", false);

  const [preview, setPreview] = useState(null);
  const [scanError, setScanError] = useState("");
  const [scanning, setScanning] = useState(false);

  const [volumes, setVolumes] = useState([]);
  const [mount, setMount] = useState(null);
  const [ffmpeg, setFfmpeg] = useState(null);

  const [phase, setPhase] = useState("idle"); // idle | running | done | error
  const [progress, setProgress] = useState(null);
  const [report, setReport] = useState(null);
  const [runError, setRunError] = useState("");
  const [ejectNote, setEjectNote] = useState("");

  const [confirm, setConfirm] = useState(null); // { kind: "run" | "format", card }
  const [busyFormat, setBusyFormat] = useState(false);

  const native = useMemo(() => (flacOk ? ["flac"] : []), [flacOk]);
  const running = phase === "running";

  /* first launch: default the music folder */
  useEffect(() => {
    if (source) return;
    api.defaultSource().then(setSource);
  }, [source, setSource]);

  /* scan the music folder whenever it, or what counts as "native", changes */
  const scan = useCallback(async () => {
    if (!source) return;
    setScanning(true);
    try {
      setPreview(await api.scan(source, native));
      setScanError("");
    } catch (e) {
      setPreview(null);
      setScanError(String(e));
    } finally {
      setScanning(false);
    }
  }, [source, native]);
  useEffect(() => {
    scan();
  }, [scan]);

  /* the card list: refresh now, on focus, and every few seconds while idle */
  const refreshVolumes = useCallback(async () => {
    try {
      const list = await api.listVolumes();
      setVolumes(list);
      setMount((current) => {
        if (current && list.some((v) => v.mount_point === current)) return current;
        const likely = list.find((v) => v.name.toUpperCase() === "MUSIC" && !v.too_large)
          ?? (list.filter((v) => !v.too_large).length === 1 ? list.find((v) => !v.too_large) : null);
        return likely ? likely.mount_point : null;
      });
    } catch {
      /* a failed poll just leaves the last list showing */
    }
  }, []);
  const pollPaused = useRef(false);
  pollPaused.current = running || busyFormat || confirm != null;
  useEffect(() => {
    refreshVolumes();
    const tick = setInterval(() => {
      if (!pollPaused.current) refreshVolumes();
    }, 4000);
    const onFocus = () => !pollPaused.current && refreshVolumes();
    window.addEventListener("focus", onFocus);
    return () => {
      clearInterval(tick);
      window.removeEventListener("focus", onFocus);
    };
  }, [refreshVolumes]);

  useEffect(() => {
    api.ffmpegStatus().then(setFfmpeg);
  }, []);

  /* progress events from the run */
  useEffect(() => {
    let off = () => {};
    let gone = false;
    api.onProgress((p) => setProgress(p)).then((unlisten) => {
      if (gone) unlisten();
      else off = unlisten;
    });
    return () => {
      gone = true;
      off();
    };
  }, []);

  const card = volumes.find((v) => v.mount_point === mount) || null;
  const needsFormat = card && !card.is_fat32;
  const needsFfmpeg = preview && preview.transcode_count > 0;
  const ffmpegMissing = needsFfmpeg && ffmpeg && !ffmpeg.ok;

  const usage = card && preview && card.total_bytes ? preview.estimated_bytes / card.total_bytes : null;
  const fits = usage != null && usage <= 0.97;
  const usageTone = usage == null ? undefined : usage > 0.97 ? "danger" : usage > 0.85 ? "warning" : "success";

  const blockers = [];
  if (!preview || preview.total === 0) blockers.push("No music found in the folder.");
  if (!card) blockers.push("Pick a card.");
  else if (card.too_large) blockers.push("That drive is too big to be a card.");
  else if (needsFormat && !fresh) blockers.push("Format the card as FAT32 first.");
  else if (usage != null && !fits) blockers.push("The music will not fit on this card.");
  if (ffmpegMissing) blockers.push("ffmpeg is needed for the conversions.");
  const canStart = blockers.length === 0 && !running && !scanning;

  /* ------------------------------------------------------------ actions -- */

  const browse = async () => {
    const picked = await api.pickFolder(source || undefined);
    if (picked) setSource(picked);
  };

  const openRunConfirm = async () => {
    setReport(null);
    setRunError("");
    setEjectNote("");
    let contents = { items: 0, bytes: 0 };
    try {
      contents = await api.cardContents(card.mount_point);
    } catch {
      /* the run re-checks; the count is only for the message */
    }
    setConfirm({ kind: "run", card, contents });
  };

  const doFormat = async (target) => {
    setBusyFormat(true);
    try {
      const v = await api.formatCard(target.whole_disk, target.name);
      await refreshVolumes();
      setMount(v.mount_point);
      return v;
    } finally {
      setBusyFormat(false);
    }
  };

  const confirmFormat = async () => {
    const target = confirm.card;
    setConfirm(null);
    try {
      await doFormat(target);
    } catch (e) {
      setRunError(String(e));
      setPhase("error");
    }
  };

  const go = async () => {
    let target = confirm.card;
    setConfirm(null);
    setPhase("running");
    setProgress({ phase: "checking", done: 0, total: 0, current: "" });
    try {
      if (fresh) {
        setProgress({ phase: "wiping", done: 0, total: 0, current: "Formatting the card as FAT32" });
        target = await doFormat(target);
      }
      const result = await api.startRun({
        source,
        target: target.mount_point,
        native_formats: native,
        mp3_quality: Number(quality),
        ffmpeg: ffmpeg?.path ?? null,
      });
      setReport(result);
      setPhase("done");
      if (eject && !result.canceled && result.written > 0) {
        try {
          await api.ejectCard(target.mount_point);
          setEjectNote("The card is ejected. It is safe to take out.");
        } catch (e) {
          setEjectNote(`Could not eject the card automatically (${e}). Eject it from Finder before removing it.`);
        }
        refreshVolumes();
      }
    } catch (e) {
      setRunError(String(e));
      setPhase("error");
    }
  };

  /* ------------------------------------------------------------- render -- */

  const pct = progress && progress.total ? progress.done / progress.total : 0;
  const indeterminate = running && (!progress || progress.phase !== "copying");

  return (
    <div className="d8">
      <Navbar variant="dark">
        <NavBrand mark={<ShuffleIcon />}>D8 Shuffler</NavBrand>
        <NavSpacer />
        {isMock ? <Badge tone="warning">Preview, no real card</Badge> : null}
      </Navbar>

      <main className="d8__main">
        <div className="d8__cols">
          <div className="d8__col">
            <Panel
              title="1 · Music folder"
              actions={
                <Button size="sm" variant="ghost" onClick={scan} disabled={running || scanning} aria-label="Scan again">
                  <RefreshIcon /> Rescan
                </Button>
              }
            >
              <InputGroup>
                <Field icon={<FolderIcon />}>
                  <input value={source} readOnly aria-label="Music folder" title={source} />
                </Field>
                <Button onClick={browse} disabled={running}>Browse</Button>
              </InputGroup>

              {scanError ? (
                <Alert tone="warning">{scanError}</Alert>
              ) : preview ? (
                <div className="stack d8__tight">
                  <div className="d8__big num">
                    {preview.total.toLocaleString()} <span className="dim">tracks · {gb(preview.source_bytes)}</span>
                  </div>
                  <div className="row d8__wrap">
                    {Object.entries(preview.by_ext).map(([ext, n]) => (
                      <Badge key={ext} tone={ext === "mp3" || ext === "wav" || (flacOk && ext === "flac") ? "success" : "info"}>
                        {ext} <span className="num">{n}</span>
                      </Badge>
                    ))}
                  </div>
                  <p className="d8__note">
                    {preview.transcode_count
                      ? `${preview.transcode_count} will be converted to MP3. Everything else is copied untouched.`
                      : "Everything plays as it is, so nothing needs converting."}
                  </p>
                </div>
              ) : (
                <p className="d8__note">Looking through the folder…</p>
              )}
            </Panel>

            <Panel title="3 · Options">
              <div className="stack d8__opts">
                <Toggle checked={flacOk} onChange={setFlacOk}>
                  My speaker plays FLAC, don’t convert it
                </Toggle>
                <div className="d8__optrow">
                  <span className="d8__label">MP3 quality</span>
                  <Segmented
                    label="MP3 quality"
                    value={quality}
                    onChange={setQuality}
                    options={[
                      { value: "0", label: "Best · V0" },
                      { value: "2", label: "Smaller · V2" },
                    ]}
                  />
                </div>
                <Toggle checked={fresh} onChange={setFresh}>
                  Reformat the card as FAT32 every time
                </Toggle>
                <Toggle checked={eject} onChange={setEject}>
                  Eject the card when finished
                </Toggle>
              </div>
              {needsFfmpeg && ffmpeg ? (
                ffmpeg.ok ? (
                  <p className="d8__note">Converting with ffmpeg at <span className="num">{ffmpeg.path}</span></p>
                ) : (
                  <Alert tone="warning" heading="ffmpeg is missing">
                    Converting needs it. In Terminal run <code>brew install ffmpeg</code>, then reopen this app.
                  </Alert>
                )
              ) : null}
            </Panel>
          </div>

          <div className="d8__col">
            <Panel
              title="2 · Card"
              actions={
                <Button size="sm" variant="ghost" onClick={refreshVolumes} disabled={running} aria-label="Refresh cards">
                  <RefreshIcon /> Refresh
                </Button>
              }
            >
              {volumes.length === 0 ? (
                <div className="d8__empty">
                  <div className="d8__big">No card found</div>
                  <p className="d8__note">Put the SD card in the reader. It shows up here by itself.</p>
                </div>
              ) : (
                <ListGroup>
                  {volumes.map((v) => (
                    <ListItem
                      key={v.mount_point}
                      action
                      current={v.mount_point === mount}
                      disabled={running || v.too_large}
                      onClick={() => setMount(v.mount_point)}
                    >
                      <span className="d8__vol">
                        <strong>{v.name}</strong>
                        <span className="num d8__volsub">
                          {v.too_large ? "Too big to be a card" : `${gb(v.total_bytes)} · ${gb(v.free_bytes)} free`}
                        </span>
                      </span>
                      <Badge tone={v.is_fat32 ? "success" : "warning"}>{v.is_fat32 ? "FAT32" : v.filesystem || "Unknown"}</Badge>
                    </ListItem>
                  ))}
                </ListGroup>
              )}

              {needsFormat ? (
                <Alert tone="warning" heading={`${card.name} is ${card.filesystem || "not FAT32"}`}>
                  The speaker reads FAT32 only. Formatting erases the whole card and sets it up the way a small
                  player expects.
                  <div className="d8__alertact">
                    <Button
                      size="sm"
                      variant="warning"
                      disabled={running || busyFormat}
                      onClick={() => setConfirm({ kind: "format", card })}
                    >
                      Format as FAT32…
                    </Button>
                  </div>
                </Alert>
              ) : card ? (
                <p className="d8__note">
                  FAT32, ready. Everything on <strong>{card.name}</strong> is replaced each time you shuffle.
                </p>
              ) : null}

              {usage != null ? (
                <div className="stack d8__fit">
                  <div className="row d8__between">
                    <span className="d8__label">Will use</span>
                    <span className="num">
                      {gb(preview.estimated_bytes)} of {gb(card.total_bytes)}
                    </span>
                  </div>
                  <Meter value={Math.min(usage, 1)} tone={usageTone} label="Space the music will use" />
                  <p className="d8__note">Estimate: converted files shrink, so the real figure is usually a little lower.</p>
                </div>
              ) : null}
            </Panel>
          </div>
        </div>

        <Panel title={phase === "done" ? "Done" : "Shuffle"} selected={running}>
          {running ? (
            <div className="stack">
              <div className="row d8__between">
                <span>{PHASE_TEXT[progress?.phase] ?? "Working"}</span>
                {progress?.phase === "copying" ? (
                  <span className="num dim">
                    {progress.done.toLocaleString()} / {progress.total.toLocaleString()}
                  </span>
                ) : null}
              </div>
              <Meter value={pct} busy={indeterminate} tall striped label="Progress" />
              <div className="d8__current num dim" title={progress?.current}>
                {progress?.current || " "}
              </div>
              <div>
                <Button variant="danger" size="sm" onClick={() => api.cancelRun()}>
                  Stop
                </Button>
              </div>
            </div>
          ) : (
            <div className="stack">
              {phase === "done" && report ? (
                <>
                  <Alert
                    tone={report.canceled ? "warning" : "success"}
                    heading={report.canceled ? "Stopped early" : `${report.written.toLocaleString()} tracks are on the card`}
                  >
                    {report.canceled
                      ? `${report.written.toLocaleString()} of ${report.planned.toLocaleString()} tracks were written before you stopped. The card is part-filled.`
                      : `${report.copied.toLocaleString()} copied as they were, ${report.transcoded.toLocaleString()} converted to MP3, in ${duration(report.elapsed_secs)}. ${ejectNote}`}
                  </Alert>
                  {report.failed.length ? (
                    <Alert tone="warning" heading={`${report.failed.length} could not be converted and were left out`}>
                      <ul className="d8__fails">
                        {report.failed.slice(0, 6).map((f) => (
                          <li key={f.source}>
                            <span className="num">{f.source.split("/").pop()}</span>
                            <span className="dim"> — {f.error.split("\n")[0]}</span>
                          </li>
                        ))}
                        {report.failed.length > 6 ? <li className="dim">…and {report.failed.length - 6} more</li> : null}
                      </ul>
                    </Alert>
                  ) : null}
                  {report.manifest_path ? (
                    <p className="d8__note">
                      Which number is which song: <span className="num">{report.manifest_path}</span>
                    </p>
                  ) : null}
                </>
              ) : null}

              {phase === "error" ? (
                <Alert heading="That did not work" onDismiss={() => setPhase("idle")}>
                  {runError}
                </Alert>
              ) : null}

              <div className="row d8__between d8__go">
                <span className="d8__why">
                  {blockers.length ? blockers[0] : `Ready: ${preview.total.toLocaleString()} tracks, shuffled fresh`}
                </span>
                <Button variant="primary" size="lg" disabled={!canStart || busyFormat} onClick={openRunConfirm}>
                  {phase === "done" ? "Shuffle again" : "Shuffle to card"}
                </Button>
              </div>
            </div>
          )}
        </Panel>
      </main>

      {/* ----------------------------------------------------------- modals -- */}

      <Modal
        open={confirm?.kind === "run"}
        title="Replace everything on this card?"
        onClose={() => setConfirm(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirm(null)}>Cancel</Button>
            <Button variant="danger" onClick={go}>
              {fresh ? "Reformat and shuffle" : "Wipe and shuffle"}
            </Button>
          </>
        }
      >
        {confirm?.kind === "run" ? (
          <div className="stack">
            <p className="d8__p">
              <strong>{confirm.card.name}</strong> ({gb(confirm.card.total_bytes)}) will be{" "}
              {fresh ? "erased and reformatted as FAT32" : "emptied"}.
              {confirm.contents.items
                ? ` That removes ${confirm.contents.items.toLocaleString()} files, ${gb(confirm.contents.bytes)}.`
                : " It looks empty already."}
            </p>
            <p className="d8__p dim">
              Then {preview.total.toLocaleString()} tracks from your music folder are written to it in a new random
              order, as numbered files in one flat list. Your music folder is not changed.
            </p>
          </div>
        ) : null}
      </Modal>

      <Modal
        open={confirm?.kind === "format"}
        title="Format this card as FAT32?"
        onClose={() => setConfirm(null)}
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirm(null)}>Cancel</Button>
            <Button variant="danger" onClick={confirmFormat}>Erase and format</Button>
          </>
        }
      >
        {confirm?.kind === "format" ? (
          <p className="d8__p">
            This erases <strong>everything</strong> on <strong>{confirm.card.name}</strong> (
            {gb(confirm.card.total_bytes)}, currently {confirm.card.filesystem || "unknown"}) and cannot be undone.
            The card is given a single FAT32 volume on an MBR partition map, which is what small players read best.
          </p>
        ) : null}
      </Modal>

      {busyFormat && !running ? (
        <div className="d8__toast">
          <Alert tone="info">Formatting the card…</Alert>
        </div>
      ) : null}
    </div>
  );
}

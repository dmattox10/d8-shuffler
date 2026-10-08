// The one place that talks to Rust.
//
// Run in a plain browser (`yarn dev` without Tauri) and every call is answered
// by a small mock, so the whole interface can be worked on without a card.
// Command arguments are camelCase here; Tauri converts them to the snake_case
// Rust parameters. The fields inside `options` are matched by serde as written.

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call(cmd, args) {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke(cmd, args);
}

const real = {
  defaultSource: () => call("default_source"),
  scan: (source, nativeFormats) => call("scan", { source, nativeFormats }),
  listVolumes: () => call("list_volumes"),
  ffmpegStatus: () => call("ffmpeg_status"),
  cardContents: (mountPoint) => call("card_contents", { mountPoint }),
  formatCard: (wholeDisk, label) => call("format_card", { wholeDisk, label }),
  ejectCard: (mountPoint) => call("eject_card", { mountPoint }),
  startRun: (options) => call("start_run", { options }),
  cancelRun: () => call("cancel_run"),
  async onProgress(handler) {
    const { listen } = await import("@tauri-apps/api/event");
    return listen("progress", (e) => handler(e.payload));
  },
  async pickFolder(defaultPath) {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const picked = await open({ directory: true, multiple: false, defaultPath });
    return typeof picked === "string" ? picked : null;
  },
};

/* ------------------------------------------------------------------- mock -- */

const wait = (ms) => new Promise((r) => setTimeout(r, ms));

function makeMock() {
  let handler = null;
  let cancelled = false;
  let volumes = [
    {
      name: "MUSIC", mount_point: "/Volumes/MUSIC", device_id: "disk4s1", whole_disk: "disk4",
      filesystem: "MS-DOS FAT32", is_fat32: true, total_bytes: 31_914_983_424,
      free_bytes: 9_400_000_000, too_large: false,
    },
    {
      name: "STICK", mount_point: "/Volumes/STICK", device_id: "disk5s2", whole_disk: "disk5",
      filesystem: "ExFAT", is_fat32: false, total_bytes: 62_000_000_000,
      free_bytes: 40_000_000_000, too_large: false,
    },
  ];
  const flac = new URLSearchParams(location.search).get("flac") === "1";
  return {
    defaultSource: async () => "/Users/dan/Music/D8",
    async scan(_source, nativeFormats) {
      await wait(250);
      const keep = nativeFormats.includes("flac");
      const flacCount = 61;
      return {
        total: 412, source_bytes: 14_800_000_000,
        estimated_bytes: keep ? 14_800_000_000 : 10_100_000_000,
        transcode_count: keep ? 0 : flacCount + 6,
        by_ext: { mp3: 338, flac: flacCount, m4a: 6, wav: 7 },
      };
    },
    listVolumes: async () => volumes,
    ffmpegStatus: async () =>
      flac
        ? { path: null, ok: false, message: "ffmpeg was not found." }
        : { path: "/opt/homebrew/bin/ffmpeg", ok: true, message: null },
    cardContents: async () => ({ items: 143, bytes: 1_020_000_000 }),
    async formatCard(_disk, label) {
      await wait(1200);
      volumes = volumes.map((v) =>
        v.whole_disk === "disk5"
          ? { ...v, name: label, filesystem: "MS-DOS FAT32", is_fat32: true, mount_point: `/Volumes/${label}` }
          : v
      );
      return volumes.find((v) => v.name === label);
    },
    ejectCard: async () => wait(300),
    async startRun() {
      cancelled = false;
      const total = 412;
      handler?.({ phase: "checking", done: 0, total: 0, current: "" });
      await wait(600);
      handler?.({ phase: "wiping", done: 0, total, current: "" });
      await wait(700);
      for (let i = 0; i < total; i += 1) {
        if (cancelled) break;
        handler?.({ phase: "copying", done: i, total, current: `Track name number ${i + 1}.flac` });
        await wait(14);
      }
      return {
        planned: total, written: cancelled ? 0 : total, transcoded: 67, copied: 345,
        failed: [{ source: "/Users/dan/Music/D8/Old/broken.flac", error: "Invalid data found when processing input" }],
        canceled: cancelled, bytes_written: 10_100_000_000, wiped_items: 143,
        manifest: [], elapsed_secs: 731, manifest_path: "~/Library/Application Support/fyi.dantheman.d8shuffler/last-run.txt",
      };
    },
    cancelRun: async () => { cancelled = true; },
    async onProgress(h) { handler = h; return () => { handler = null; }; },
    pickFolder: async () => "/Users/dan/Music/D8",
  };
}

export const api = inTauri ? real : makeMock();
export const isMock = !inTauri;

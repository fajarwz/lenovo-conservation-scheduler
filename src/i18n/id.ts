import type { enUS } from "./en-US";

// Typed as the English dictionary, so a missing or misspelled key is a compile error.
export const id: typeof enUS = {
  "app.subtitle": "Mengalihkan Mode Konservasi Lenovo sesuai jadwal Anda.",
  "app.unavailable": "Kontrol baterai Lenovo tidak tersedia di perangkat ini.",

  "common.yes": "Ya",
  "common.no": "Tidak",
  "common.on": "AKTIF",
  "common.off": "NONAKTIF",
  "common.full": "Penuh",
  "common.paused": "Dijeda",
  "common.unavailable": "Tidak tersedia",
  "common.loading": "Memuat…",

  "status.saving": "Menyimpan…",
  "status.saved": "Tersimpan",
  "load.failed": "Gagal memuat pengaturan: {error}",

  "battery.title": "Baterai",
  "battery.charge": "Isi baterai",
  "battery.power": "Sumber daya",
  "battery.charging": "Mengisi",
  "battery.conservation": "Mode Konservasi",
  "battery.pluggedIn": "Tersambung listrik",
  "battery.onBattery": "Pakai baterai",
  "battery.unavailableFallback": "Mode konservasi tidak tersedia.",

  "scheduler.title": "Penjadwal",
  "scheduler.add": "Tambah jadwal",
  "scheduler.apply": "Terapkan jadwal secara otomatis",
  "scheduler.next": "Berikutnya: {action} · {day} · {time}",
  "scheduler.nothing": "Belum ada jadwal.",
  "scheduler.off": "Jadwal sedang nonaktif, jadi tidak ada yang berubah sendiri.",
  "scheduler.empty": "Belum ada jadwal.",
  "scheduler.presetWeekdays": "Sen–Jum",
  "scheduler.presetAll": "Semua",
  "scheduler.removed": "Jadwal dihapus.",
  "scheduler.undo": "Urungkan",
  "scheduler.on": "Aktif",
  "scheduler.time": "Waktu",
  "scheduler.days": "Hari",
  "scheduler.action": "Aksi",

  "action.on": "Konservasi AKTIF",
  "action.off": "Konservasi NONAKTIF",
  "row.delete": "Hapus",

  "settings.title": "Pengaturan",
  "settings.startWithWindows": "Jalankan saat Windows mulai",
  "settings.notify": "Tampilkan notifikasi saat mode berubah",
  "settings.language": "Bahasa",
  "settings.timeFormat": "Format waktu",

  "timeFormat.24h": "24 jam (17:00)",
  "timeFormat.12h": "12 jam (5:00 PM)",

  "timeField.hour": "Jam",
  "timeField.minute": "Menit",
  "timeField.amPm": "AM atau PM",

  "day.long.monday": "Senin",
  "day.long.tuesday": "Selasa",
  "day.long.wednesday": "Rabu",
  "day.long.thursday": "Kamis",
  "day.long.friday": "Jumat",
  "day.long.saturday": "Sabtu",
  "day.long.sunday": "Minggu",

  "day.monday": "Sn",
  "day.tuesday": "Sl",
  "day.wednesday": "Rb",
  "day.thursday": "Km",
  "day.friday": "Jm",
  "day.saturday": "Sb",
  "day.sunday": "Mi",
};

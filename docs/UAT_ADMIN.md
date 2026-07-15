# UAT Admin Checklist

Gunakan akun `admin` untuk seluruh skenario tulis. Ulangi skenario bertanda RBAC
dengan akun `viewer`; viewer hanya boleh membaca data dan tidak boleh melihat atau
menjalankan aksi yang mengubah data.

| ID | Area | Skenario uji | Hasil yang diharapkan | Status |
| --- | --- | --- | --- | --- |
| AUTH-01 | Login | Login dengan kredensial admin yang valid | Masuk ke Dashboard dan sesi tersimpan | ☐ |
| AUTH-02 | Login | Login dengan password salah | Pesan kredensial tidak valid, tidak ada sesi dibuat | ☐ |
| AUTH-03 | Sesi | Buka ulang halaman setelah login | Sesi tetap aktif dan data dimuat | ☐ |
| AUTH-04 | Logout | Logout lalu buka halaman Dashboard | Kembali ke halaman Login | ☐ |
| DASH-01 | Dashboard | Buat/kirim webhook baru | Event baru tampil tanpa refresh penuh; status dan source benar | ☐ |
| DASH-02 | Dashboard | Cari event berdasarkan ID, source, atau payload | Hanya event yang cocok tampil | ☐ |
| DASH-03 | Dashboard | Filter status, source, dan rentang tanggal | Hasil dan jumlah event sesuai filter | ☐ |
| DASH-04 | Dashboard | Hapus semua filter dan refresh | Daftar kembali ke kondisi awal | ☐ |
| DASH-05 | Dashboard | Pilih beberapa event lalu bulk retry | Hanya event terpilih yang dijadwalkan ulang | ☐ |
| DASH-06 | Dashboard | Pilih beberapa event lalu bulk delete | Dialog konfirmasi tampil dan event hilang setelah dikonfirmasi | ☐ |
| EVENT-01 | Detail event | Buka detail event dari Dashboard | Payload, target URL, status, dan metadata sesuai event | ☐ |
| EVENT-02 | Detail event | Lihat riwayat attempt event gagal | Attempt, waktu, response/error, dan nomor percobaan tampil benar | ☐ |
| EVENT-03 | Detail event | Retry event gagal | Status berubah/diperbarui dan attempt baru tercatat | ☐ |
| EVENT-04 | Detail event | Ack atau hapus event | Aksi berhasil setelah konfirmasi dan daftar diperbarui | ☐ |
| RULE-01 | Providers | Tambah provider dengan source, target URL, method, dan header valid | Provider tampil dan webhook dari source tersebut diteruskan | ☐ |
| RULE-02 | Providers | Ubah provider yang ada | Perubahan tersimpan dan dipakai untuk webhook berikutnya | ☐ |
| RULE-03 | Providers | Masukkan target URL/header tidak valid | Validasi form menolak input dan tidak membuat rule | ☐ |
| RULE-04 | Providers | Hapus provider | Dialog konfirmasi tampil; provider tidak lagi ada setelah konfirmasi | ☐ |
| HOOK-01 | Hooks | Tambah hook forwarding | Hook tersimpan dan forwarding tambahan berjalan | ☐ |
| HOOK-02 | Hooks | Edit atau hapus hook | Data berubah/hilang sesuai aksi | ☐ |
| SEND-01 | Send | Kirim payload JSON via provider yang dipilih | Event dibuat dan hasil pengiriman ditampilkan | ☐ |
| SEND-02 | Send | Kirim custom source, target HTTPS, dan payload valid | Pengiriman berhasil dan event masuk Dashboard | ☐ |
| SEND-03 | Send | Kirim target URL atau JSON tidak valid | Validasi lokal tampil; request tidak dikirim | ☐ |
| METRIC-01 | Metrics | Ganti periode 24 jam, 7 hari, dan 30 hari | Kartu statistik dan grafik berubah sesuai periode | ☐ |
| ACTIVITY-01 | Activity | Lakukan create/edit/delete/retry lalu buka Activity | Audit mencatat actor, action, waktu, dan resource yang benar | ☐ |
| USER-01 | Users | Lihat daftar user | Username, role, dan informasi user tampil benar | ☐ |
| USER-02 | Users | Ubah role user admin/viewer | Role tersimpan; akses menu/aksi berubah setelah login ulang | ☐ |
| ORG-01 | Organization | Lihat usage dan limit organisasi | Pemakaian event, user, dan domain sesuai data backend | ☐ |
| ORG-02 | Domains | Tambah domain valid | Instruksi verifikasi tampil dan domain tersimpan | ☐ |
| ORG-03 | Domains | Verifikasi lalu hapus domain | Status verifikasi diperbarui; domain hilang setelah hapus | ☐ |
| TOKEN-01 | Settings/API Tokens | Buat API token dengan nama perangkat | Token hanya tampil sekali, lalu muncul di daftar perangkat/token | ☐ |
| TOKEN-02 | Settings/API Tokens | Gunakan token untuk endpoint API yang diizinkan | Request berhasil sesuai role pemilik token | ☐ |
| TOKEN-03 | Settings/API Tokens | Revoke API token lalu gunakan kembali | Token tidak lagi dapat mengakses API | ☐ |
| SET-01 | Settings | Lihat health dan profil pengguna | Status backend dan role pengguna sesuai kondisi aktual | ☐ |
| RBAC-01 | Viewer | Login sebagai viewer | Menu Users, Organization, dan Send tidak tampil | ☐ |
| RBAC-02 | Viewer | Akses URL aksi tulis atau panggil API mutasi langsung | Ditolak (`403`/unauthorized); data tidak berubah | ☐ |
| RBAC-03 | Viewer | Buka Dashboard, Providers, Hooks, Metrics, dan Activity | Data baca dapat dilihat tanpa aksi mutasi | ☐ |
| PLATFORM-01 | Platform operator | Buka halaman Platform dengan operator | Overview dan daftar organisasi dapat dilihat | ☐ |
| PLATFORM-02 | Platform operator | Provision organisasi dan ubah subscription | Organisasi baru tersimpan; limit/status subscription terbarui | ☐ |
| PLATFORM-03 | Non-operator | Akses `/platform` | Dialihkan ke Dashboard | ☐ |

## Data uji minimum

| Data | Nilai yang disarankan |
| --- | --- |
| Source provider | `uat-github` |
| Target berhasil | Endpoint receiver yang membalas `200` |
| Target gagal | Endpoint receiver yang membalas `500` atau tidak dapat dijangkau |
| Payload valid | `{ "event": "uat", "id": "case-001" }` |
| Payload tidak valid | `{ event: uat }` |
| Akun role | Satu `admin`, satu `viewer`, dan satu platform operator bila fitur platform diuji |

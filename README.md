# ladder

کلاینت خط فرمان برای دو کار، با بودجهٔ جدا به ثانیه:

1. نردبان سایفون: دامین‌فرانتینگ، آی‌پی تمیز، اتصال مستقیم، کاندوئیت، کاندوئیت شخصی، پروکسی بالادست.
2. رلهٔ ویندسکرایب: پروتکل‌های TCP (OpenVPN TCP، Stealth، WStunnel) به `127.0.0.1` وصل می‌شوند و این برنامه همان اتصال را از داخل یک SOCKS کلودفلر (WARP یا ورکر) به آی‌پی واقعی سرور می‌برد.

رمز و خودِ تونل مال این برنامه نیست. سایفون با باینری رسمی [`psiphon-tunnel-core`](https://github.com/Psiphon-Labs/psiphon-tunnel-core) بالا می‌آید. ویندسکرایب هم به کانفیگ خودتان (لیست سرور export‌شده و یک سشن WARP) وصل می‌شود. راز امضای API رسمی WSNet اینجا کپی نشده.

## بیلد

```bash
cargo test --locked
cargo build --release
```

روی گیتهاب، workflow داخل `.github/workflows/ci.yml` همین تست را اجرا می‌کند.

## بودجه

در `config.toml` (از روی `config.example.toml`):

```toml
[budgets]
A = 60   # دامین‌فرانتینگ
E = 45   # آی‌پی تمیز
D = 45   # مستقیم
C = 75   # کاندوئیت
P = 90   # کاندوئیت شخصی، فقط اگر pin شود
X = 45   # هر پروکسی، بعدش بقیهٔ نردبان اگر هیچ‌کدام تونل را نبرد
```

`psiphon.pinned = ""` یعنی خودکار. آخرین روشی که SOCKS محلی‌اش جواب داده دفعهٔ بعد اول امتحان می‌شود. `P` هیچ‌وقت خودکار نیست.

## سایفون

فایل `psiphon.base.json` را از بیلد خودتان از هستهٔ رسمی بگذارید (SponsorId، آدرس لیست سرور، کلید امضا). این برنامه فقط روش را روی همان JSON می‌گذارد:

| روش | چه چیزی به هسته داده می‌شود |
|---|---|
| A | فقط `FRONTED-MEEK-OSSH` و `FRONTED-MEEK-HTTP-OSSH` |
| E | همان، به‌علاوهٔ `DNSResolverPreresolvedIPAddressCIDRs` از اسکن |
| D | بدون محدودیت پروتکل |
| C | فقط `INPROXY-WEBRTC-OSSH` |
| P | همان، با `InproxyClientPersonalCompartmentID` |
| X | `UpstreamProxyURL` |

```bash
cp config.example.toml config.toml
# نام‌های فرانت را در cleanip.name_templates بگذارید، بعد:
./target/release/ladder psiphon scan
./target/release/ladder psiphon proxies
./target/release/ladder psiphon run
```

`scan` نام‌ها را روی DNS همین خط resolve می‌کند، همسایه‌های /24 یا خوشهٔ ۲۵۶تایی IPv6 را می‌سازد، TLS با SNI تصادفی می‌گیرد و گواهی را با `cert_needles` چک می‌کند. اگر `meek_hosts` پر باشد، لبه‌ای که TLS را تمام کند و بعد ساکت بماند دور ریخته می‌شود. خروجی `data/psiphon/clean-ips.json` است.

پروکسی‌ها در `proxies.txt`، هر خط `http://host:port` یا `socks5://host:port`. تست سریع فقط رد شدن TCP است. حمل تونل را خود هستهٔ سایفون در بودجهٔ X ثابت می‌کند.

وقتی وصل شد، SOCKS روی `127.0.0.1` و پورت `psiphon.local_socks_port` می‌ماند.

## ویندسکرایب از کلودفلر

`examples/servers.example.json` را با خروجی خودتان عوض کنید. انتخاب آدرس مثل کلاینت رسمی است: WireGuard و Stealth از `ip3`، OpenVPN از `ip2`، WStunnel از `ip`. کشورهای `prefer_countries` اول می‌آیند.

یک سشن WARP (یا SOCKS ورکر) را جدا بالا بیاورید و آدرسش را در `windscribe.socks` بگذارید. دو سشن با یک کلید WARP، اولی را از کار می‌اندازد؛ این رله سشن دوم نمی‌سازد.

```bash
./target/release/ladder windscribe plan
./target/release/ladder windscribe relay --proto openvpn-tcp
```

بعد ریموت OpenVPN / Stealth / WStunnel را بگذارید روی آدرسی که چاپ می‌شود. گواهی سرور ویندسکرایب سر‌به‌سر چک می‌شود؛ رله فقط لولهٔ TCP است.

WireGuard و OpenVPN UDP پروتکل UDP هستند. این رله آن‌ها را حمل نمی‌کند. همان یک سشن WARP باید UDP آن‌ها را ببرد.

## چه چیزی عمداً داخل مخزن نیست

- اکانت، پسورد، کلید WARP، SponsorId سایفون، لیست سرور امضاشده
- باینری `libwsproxy` / `wsnet` ویندسکرایب
- پیاده‌سازی دوبارهٔ پروتکل سایفون یا امضای API ویندسکرایب

## اپ اندروید

پوشهٔ `android/` یک اپ اندروید (Kotlin، بدون کتابخانهٔ اضافه) است که باینری Rust همین پروژه را به‌صورت `libladder.so` داخل APK می‌گذارد و با یک سرویس پیش‌زمینه اجرا می‌کند. خروجی SOCKS محلی است (`127.0.0.1:1080`)، نه VPN سراسری.

بیلد با گیتهاب: هر push، workflow به نام `ci` را اجرا می‌کند و در Artifacts فایل `ladder-apk` را می‌گذارد (zip، داخلش APK).

داخل اپ:

- **وارد کردن فایل**: باینری `psiphon-tunnel-core` برای arm64 (خودتان بدهید، اینجا نیست)، `psiphon.base.json`، `proxies.txt`، `servers.json`.
- **ویرایش فایل‌ها**: همان‌ها و `config.toml` را مستقیم ویرایش یا پیست کنید.
- **شروع / توقف**: یکی از حالت‌ها (سایفون اجرا / اسکن / تست پروکسی، ویندسکرایب رله) را انتخاب کنید.

اگر `libpsiphon.so` را در `android/app/src/main/jniLibs/arm64-v8a/` بگذارید، داخل APK می‌رود و دیگر نیازی به وارد کردن نیست.

کلید امضا (`android/ladder.keystore`) عمداً در مخزن است تا هر بیلد روی قبلی نصب شود. برای انتشار عمومی عوضش کنید.

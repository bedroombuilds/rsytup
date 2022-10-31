# rsytup - Rust YouTube uploader

## features

- make a thumbnail based on the video, a TTF and a watermark PNG
- setting title based on video filename
- description from cmd-line arg
- easily set publish-date
- writes tags, language info and more
- add the Video it to a Playlist
- edit all uploaded videos metadata (e.g. add text to the description)

## Quickstart

Please note that you need a client secret to successfully connect to YouTube API.
Download it from [Developer Console](https://console.developers.google.com) and
place it into the file `client_secret.json` in your working directory so that
`rsytup` can find it.

Run with `RUST_LOG=debug` in order to see an accurate record of HTTP requests
being sent and received.

```bash
# show command line options
rsytup --help
# list the current top 5 videos on youtube (best to verify your client_secret
# and token_cache
rsytup list --yt-top5
# upload practical example: uploading the video, setting keywords, title and adding to a playlist
cargo run -- upload -f "29. Pattern matching revisited.mov" \
    -d "revisiting pattern matching now that Python 3.10 has been released" \
    --playlist-id "PLEIv4NBmh-your-random-id" \
    --keywords "rust,tutorial,python,structural,pattern,matching" \
    --title "29. Pattern matching - From Python to Rust"
```

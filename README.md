# transcoder-av1-example 1

## Summary
An example transcoder server that can convert h264 video format to AV1 codec. AV1 is an open-source, royalty-free video codec that has better compression than h264 with smaller file sizes and hence less bandwidth. There is also support for higher resolutions and frame rates.
AV1 is already supported by a number of popular browsers, including Google Chrome, Mozilla Firefox, and Microsoft Edge. It is also supported by a growing number of video players and streaming services.

## Encoding
AV1 compression requires much more complexity than h264. Even with top of the line CPU, encoding rates are much less than real-time. Hardware encoding is realistically required and this is available on GPUs such as NVIDIA RTX 4000 series, NVIDIA A6000 or Intel Arc GPUs.

## Technology used

The transcoder network integrates to S5 for its content delivery network (CDN) and its ability to store content to Sia cloud storage.

S5 is a content-addressed storage network similar to IPFS, but with some new concepts and ideas to make it more efficient and powerful.
https://github.com/s5-dev

Sia is a decentralized cloud storage platform that uses blockchain technology to store data https://sia.tech/. It is designed to be more secure, reliable, and affordable than traditional cloud storage providers. Sia encrypts and distributes your files across a decentralized network of hosts. This means that your data is not stored in a single location, and it is not accessible to anyone who does not have the encryption key.

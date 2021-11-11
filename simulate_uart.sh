qemu-system-arm \
    -machine sabrelite \
	-nographic \
    -s \
	-m size=1024M  \
	-kernel artifacts/debug/kernel \
	-initrd artifacts/debug/feL4img \
    -serial telnet:0.0.0.0:8888,server,nowait \
    -serial mon:stdio

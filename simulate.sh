qemu-system-arm \
    -machine sabrelite \
	  -nographic \
    -s \
    -chardev stdio,mux=on,id=char0 \
    -mon chardev=char0,mode=readline \
    -serial chardev:char0 \
    -serial chardev:char0 \
	  -m size=1024M  \
	  -kernel artifacts/debug/kernel \
	  -initrd artifacts/debug/feL4img



	  # -serial null \
	      # -serial mon:stdio \

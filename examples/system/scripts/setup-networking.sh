#!/usr/bin/env bash

INTERFACE="qemu-net"
HWADDR="00:00:5e:01:23:ff"
ADDR="192.0.2.2/24"
ROUTE="192.0.2.0/24"

STOPPED=0
trap ctrl_c INT TERM

ctrl_c() {
    STOPPED=1
}

echo "Adding interface '$INTERFACE'"

ip tuntap add $INTERFACE mode tap

ip link set dev $INTERFACE up

ip link set dev $INTERFACE address $HWADDR

ip address add $ADDR dev $INTERFACE

ip route add $ROUTE dev $INTERFACE > /dev/null 2>&1

while [ $STOPPED -eq 0 ]; do
    sleep 1d
done

ip link set $INTERFACE down

echo "Deleting $INTERFACE"

ip tuntap del $INTERFACE mode tap

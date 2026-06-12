<?php
function getMethodName(array $ddata = []): void {
    if (\array_key_exists("redirect", $ddata)) {
        return;
    }
    if (random_int(0, 1)) {
        $ddata["type"] = "test";
    }
    /** @psalm-check-type-exact $ddata = array<array-key, mixed> */
}

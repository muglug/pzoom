<?php
function dummy_taintable(string $in): string {
    return $in;
}

/**
 * @psalm-flow proxy dummy_taintable($r) -> return
 */
function some_stub(string $r): string {}

$r = $_GET["untrusted"];

echo some_stub($r);

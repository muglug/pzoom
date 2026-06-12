<?php
class dummy {
    public function taintable(string $in): string {
        return $in;
    }
}

/**
 * @psalm-flow proxy dummy::taintable($r) -> return
 */
function some_stub(string $r): string {}

$r = $_GET["untrusted"];

echo some_stub($r);

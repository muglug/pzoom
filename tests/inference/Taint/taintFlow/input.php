<?php
/**
 * @psalm-flow ($r) -> return
 */
function some_stub(string $r): string { return ""; }

$r = $_GET["untrusted"];

echo some_stub($r);

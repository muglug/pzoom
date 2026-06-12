<?php
/**
 * @psalm-taint-sink callable $in
 */
function dummy_taint_sink(string $in): void {}

/**
 * @psalm-flow proxy dummy_taint_sink($r)
 */
function some_stub(string $r): string {}

$r = $_GET["untrusted"];

some_stub($r);

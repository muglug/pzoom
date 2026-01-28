<?php
class A {}

$from = rand(0, 1) ? new A() : null;
$to = rand(0, 1) ? new A() : null;

if ($from === null && $to === null) {
} elseif ($from !== null) {
} elseif ($to !== null) {}

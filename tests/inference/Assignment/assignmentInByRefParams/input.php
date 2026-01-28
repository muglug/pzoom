<?php
function foo(?string $s, ?string $t): void {}
foo($s = null, $t = null);
echo $s;
echo $t;

function foo2(?string &$u, ?string &$v): void {}
foo2($u = null, $v = null);
echo $u;
echo $v;

$read = [fopen('php://stdin', 'rb')];
$return = stream_select($read, $w = null, $e = null, 0);
echo $w;
echo $e;

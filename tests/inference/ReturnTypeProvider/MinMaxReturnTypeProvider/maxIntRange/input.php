<?php
$headers = fgetcsv(fopen("test.txt", "r"));
$h0 = $h1 = null;
foreach($headers as $i => $v) {
    if ($v === "") $h0 = $i;
    if ($v === "") $h1 = $i;
}
if ($h0 === null || $h1 === null) throw new \Exception();

$min = min($h0, $h1);
$max = max($h0, $h1);
            

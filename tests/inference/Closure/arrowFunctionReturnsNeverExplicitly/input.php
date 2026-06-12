<?php
$bar = ["foo", "bar"];

$bam = array_map(
    /** @return never */
    fn(string $a) => die(),
    $bar
);

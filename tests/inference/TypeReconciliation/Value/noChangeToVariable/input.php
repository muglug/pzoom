<?php
$i = 0;

$a = function() use ($i) : void {
    $i++;
};

$a();

if ($i === 0) {}

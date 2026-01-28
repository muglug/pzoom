<?php
function foo($f) : void {
    $i = 0;
    $f->add(function() use (&$i) : void {
        if (rand(0, 1)) $i++;
    });
    if ($i === 0) {}
}
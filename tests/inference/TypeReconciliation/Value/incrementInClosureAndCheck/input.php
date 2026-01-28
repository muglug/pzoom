<?php
$i = 0;
$a = function() use (&$i) : void {
    if (rand(0, 1)) {
        $i++;
    }
};
$a();
if ($i === 0) {}
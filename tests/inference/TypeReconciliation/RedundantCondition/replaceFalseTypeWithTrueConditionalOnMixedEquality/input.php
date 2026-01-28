<?php
function getData() {
    return rand(0, 1) ? [1, 2, 3] : false;
}

$a = false;

while ($i = getData()) {
    if (!$a && $i[0] === 2) {
        $a = true;
    }

    if ($a === false) {}
}
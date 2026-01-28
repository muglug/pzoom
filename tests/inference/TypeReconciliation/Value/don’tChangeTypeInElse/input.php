<?php
/** @var 0|string */
$x = 0;
$y = rand(0, 1) ? 0 : 1;
if ($x !== $y) {
} else {
    if (!is_string($x)) {
        chr($x);
    }
}

/** @var int|string */
$x = 0;
if ($x !== $y) {
} else {
    if (!is_string($x)) {
        chr($x);
    }
}
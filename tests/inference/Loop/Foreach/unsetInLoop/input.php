<?php
$a = null;

foreach ([1, 2, 3] as $i) {
    $a = $i;
    unset($i);
}

<?php
$a = ["b", "c", "d"];
array_pop($a);
while ($a) {
    $letter = array_pop($a);
    if (!$a) {}
}

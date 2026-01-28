<?php
/**
 * @psalm-suppress InvalidReturnType
 * @return int[]
 */
function getIntArray() {}

$x = array();
foreach (getIntArray() as $id) {
    $x[] = "TEXT";
    $x[] = "some_" . $id;
}

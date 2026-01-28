<?php
/**
 * @psalm-suppress InvalidReturnType
 * @return string[]
 */
function getStringArray() {}

$x = array();
foreach (getStringArray() as $id) {
    $x[] = "0";
    $x[] = "some_" . $id;
}

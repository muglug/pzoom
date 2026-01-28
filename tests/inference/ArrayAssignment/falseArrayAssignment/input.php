<?php
function foo(): array {
    $array = [];
    $array[false] = "";
    echo $array[0];
    return $array;
}

<?php
function bar(string $f) : void {
    $filter = rand(0, 1) ? explode(",", $f) : [$f];
    unset($filter[rand(0, 1)]);
    if ($filter) {}
}
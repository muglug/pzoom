<?php
/** @var array{s:array<mixed, array<int, string>|string>} */
$doc = [];

if (!is_array($doc["s"]["t"])) {
    $doc["s"]["t"] = [$doc["s"]["t"]];
}
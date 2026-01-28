<?php
/** @var array<string, array<int, string>> */
$a = null;

if (isset($a["b"]) || isset($a["c"])) {
    $all_params = ($a["b"] ?? []) + ($a["c"] ?? []);
}
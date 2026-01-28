<?php
function getArray() : array {
    return rand(0, 1) ? ["attr" => []] : [];
}

$out = getArray();
$out["attr"] = (array) ($out["attr"] ?? []);
$out["attr"]["bar"] = 1;

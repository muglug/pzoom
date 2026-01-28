<?php
function maybe_returns_array(): ?array {
    if (rand() % 2 > 0) {
        return ["key" => "value"];
    }
    if (rand() % 3 > 0) {
        throw new Exception("An exception occurred");
    }
    return null;
}

function try_catch_check(): array {
    $arr = null;
    try {
        $arr = maybe_returns_array();
        if (!$arr) { return [];  }
    } catch (Exception $e) {
        if (!$arr) { return []; }
    }
    return $arr;
}
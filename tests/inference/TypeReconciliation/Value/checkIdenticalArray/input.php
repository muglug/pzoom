<?php
/** @psalm-suppress MixedAssignment */
$array = json_decode(file_get_contents('php://stdin'));

if (is_array($array)) {
    $filtered = array_filter($array, fn ($value) => \is_string($value));

    if ($array === $filtered) {
        foreach ($array as $obj) {
            echo strlen($obj);
        }
    }
}
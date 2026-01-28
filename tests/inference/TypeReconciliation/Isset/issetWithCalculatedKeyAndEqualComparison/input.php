<?php
/** @var array<string, string> $array */
$array = [];

function sameString(string $string): string {
    return $string;
}

if (isset($array[sameString("key")]) === false) {
    throw new \LogicException("No such key");
}
$value = $array[sameString("key")];
                    

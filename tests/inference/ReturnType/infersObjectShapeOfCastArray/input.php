<?php
/**
 * @return array{a:1}
 */
function returnsArray(): array {
    return ["a" => 1];
}

$obj = (object)returnsArray();

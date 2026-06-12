<?php
namespace N;

/**
 * @psalm-param array{key?:string} $p
 */
function f(array $p): void
{
    echo isset($p["key"]) ? $p["key"] : "";
}

<?php
/**
 * @param array{host?:string} $opts
 * @return array{host:string|int}
 */
function b(array $opts): array {
    return array_merge(["host" => 5], $opts);
}

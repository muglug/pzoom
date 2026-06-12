<?php
/**
 * @param array{host?:string} $opts
 * @return array{host:string|int}
 */
function a(array $opts): array {
    return $opts + ["host" => 5];
}

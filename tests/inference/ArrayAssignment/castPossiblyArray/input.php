<?php
/**
 * @psalm-param string|list<string> $a
 * @return list<string>
 */
function addHeaders($a): array {
    return (array)$a;
}

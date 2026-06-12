<?php
/**
 * @param non-empty-string $d
 * @return non-empty-list<string>
 */
function exploder(string $d, string $s) : array {
    return explode($d, $s);
}

<?php
/**
 * @param array<int, string> $str
 * @return array<int, string>
 */
function test2(array $str): array {
    return mb_convert_encoding($str, "UTF-8", "UTF-8");
}

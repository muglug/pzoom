<?php
/**
 * @param array<int,string> $strings
 * @return array<int,string>
 */
function filter(array $strings): array {
   return preg_grep("/search/", $strings, PREG_GREP_INVERT);
}

<?php
function f(string $s): string {
    preg_match_all('/(a)(b)/', $s, $matches, PREG_OFFSET_CAPTURE | PREG_SET_ORDER);
    $out = '';
    foreach ($matches as $match) {
        $out .= $match[1][0] . ':' . (string) $match[2][1];
    }
    return $out;
}
echo f("ab");

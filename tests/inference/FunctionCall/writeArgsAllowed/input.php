<?php
/**
 * @param non-empty-string $pattern
 * @param 0|256|512|768 $flags
 * @return false|int
 */
function safeMatch(string $pattern, string $subject, ?array $matches = null, int $flags = 0) {
    return \preg_match($pattern, $subject, $matches, $flags);
}

safeMatch("/a/", "b");

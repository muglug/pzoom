<?php
/**
 * @return array<list<string>>
 */
function extractUsernames(string $input): array {
    preg_match_all('/([a-zA-Z])*/', $input, $matches);

    return $matches;
}

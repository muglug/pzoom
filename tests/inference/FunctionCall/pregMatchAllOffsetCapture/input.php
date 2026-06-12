<?php
function foo(string $input): array {
    preg_match_all('/([a-zA-Z])*/', $input, $matches, PREG_OFFSET_CAPTURE);

    return $matches[0];
}

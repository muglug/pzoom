<?php

/** @return non-empty-list<string> */
function f(): array
{
    return explode(DIRECTORY_SEPARATOR, 'a/b' . PHP_EOL);
}

/** @return int<1, max> */
function g(): int
{
    return PHP_VERSION_ID;
}

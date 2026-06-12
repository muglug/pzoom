<?php
/**
 * @param non-empty-array<string, string> $files
 */
function foo(array $files): void
{
    $file = reset($files);
    foreach ($files as $file) {
        strlen($file);
        $file = 0;
    }
}

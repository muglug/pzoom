<?php
function foo(string $dir) : void {
    $iterator = new \RecursiveIteratorIterator(
        new \RecursiveDirectoryIterator($dir)
    );

    while ($iterator->valid()) {
        if (!$iterator->isDot() && $iterator->isLink()) {}

        $iterator->next();
    }
}

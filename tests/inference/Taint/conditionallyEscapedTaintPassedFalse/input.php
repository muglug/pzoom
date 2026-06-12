<?php
/**
 * @psalm-taint-escape ($escape is true ? "html" : null)
 */
function foo(string $string, bool $escape = true): string {
    if ($escape) {
        $string = htmlspecialchars($string);
    }

    return $string;
}

echo foo($_GET["foo"], false);

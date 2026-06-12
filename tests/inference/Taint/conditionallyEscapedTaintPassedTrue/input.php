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
/** @psalm-suppress PossiblyInvalidArgument */
echo foo($_GET["foo"], true);
/** @psalm-suppress PossiblyInvalidArgument */
echo foo($_GET["foo"]);

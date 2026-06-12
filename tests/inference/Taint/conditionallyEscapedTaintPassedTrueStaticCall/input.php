<?php
class U {
    /**
     * @psalm-taint-escape ($escape is true ? "html" : null)
     */
    public static function foo(string $string, bool $escape = true): string {
        if ($escape) {
            $string = htmlspecialchars($string);
        }

        return $string;
    }
}

echo U::foo($_GET["foo"], true);
echo U::foo($_GET["foo"]);

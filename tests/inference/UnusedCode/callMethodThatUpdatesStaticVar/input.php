<?php
final class References {
    /**
     * @var array<string, string>
     */
    public static $foo = [];

    /**
     * @param array<string, string> $map
     */
    public function bar(array $map) : void {
        self::$foo += $map;
    }
}

(new References)->bar(["a" => "b"]);

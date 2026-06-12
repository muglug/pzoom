<?php
namespace MyNS;

class Example {
    /** @return array<int,example> */
    public static function test() : array {
        return [new Example()];
    }

    /** @return example */
    public static function instance() {
        return new Example();
    }
}

<?php

final class a {
    public static self $a;
    public static function get(): a {
        return new a;
    }
}

final class b {
    public function test(): a {
        return new a;
    }
}

function process(b $handler): a {
    if (\extension_loaded("fdsfdsfd")) {
        return $handler->test();
    }
    if (\extension_loaded("fdsfdsfd")) {
        return a::$a;
    }
    if (\extension_loaded("fdsfdsfd")) {
        return a::get();
    }
    return $handler->test();
}

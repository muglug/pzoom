<?php
namespace {
    class_alias("Bar\F1", "Bar\F2");
}

namespace Bar {
    class F1 {
        public static function baz() : void {}
    }
}

namespace {
    Bar\F2::baz();
}

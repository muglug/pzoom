<?php
namespace Foo {
    /**
     * @method  void setInteger(\Closure $c)
     */
    class Child {
        public function __call(string $s, array $args) {}
    }
}

namespace {
    $child = new Foo\Child();
    $child->setInteger(function() : void {});
}

<?php
if (class_exists(Foo::class)) {
    /** @return mixed */
    fn() => Foo::bar(23, []);
}

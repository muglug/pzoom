<?php
if (class_exists(Foo::class)) {
    /** @return mixed */
    function () {
        return Foo::bar(23, []);
    };
}

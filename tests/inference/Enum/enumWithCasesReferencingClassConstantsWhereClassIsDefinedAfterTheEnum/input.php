<?php
enum Bar: string {
    case FOO = Foo::FOO;
}
class Foo {
    const FOO = "foo";
}
$a = Bar::FOO->value;

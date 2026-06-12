<?php
enum Bar: string {
    case BAR = Foo::FOO->value;
}
enum Foo: string {
    case FOO = "foo";
}
$a = Bar::BAR->value;

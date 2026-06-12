<?php
class FooBar {
    public const FOO = 'foo';
    public const BAR = 2;
}

enum FooEnum: string {
    case FOO = FooBar::FOO;
}

enum BarEnum: int {
    case BAR = FooBar::BAR;
}

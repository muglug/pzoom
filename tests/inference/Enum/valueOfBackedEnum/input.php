<?php
enum StringEnum: string {
    case FOO = 'foo';
    case BAR = 'bar';
}

enum IntEnum: int {
    case FOO = 1;
    case BAR = 2;
}

/** @var value-of<StringEnum::FOO> $string */
$string = '';
/** @var value-of<StringEnum::*> $anyString */
$anyString = '';

/** @var value-of<IntEnum::FOO> $int */
$int = 0;
/** @var value-of<IntEnum::*> $anyInt */
$anyInt = 0;

<?php
enum SomeType: string
{
    case FOO = "FOO";
    case BAR = "BAR";
}

function getSomething(string $moduleString): int
{
    return match ($moduleString) {
        SomeType::FOO->value => 1,
        SomeType::BAR->value => 2,
    };
}

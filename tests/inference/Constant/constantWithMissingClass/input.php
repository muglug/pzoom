<?php
class Subject
{
    public const DATA = [
        MissingClass::TAG_DATA,
    ];

    public function execute(): void
    {
        /** @psalm-suppress InvalidArrayOffset */
        if (self::DATA["a"]);
    }
}

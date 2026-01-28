<?php
final class Date
{
    /**
     * @param non-empty-string $tzString
     * @psalm-pure
     */
    public static function timeZone(string $tzString) : DateTimeZone
    {
        return new \DateTimeZone($tzString);
    }
}

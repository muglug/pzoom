<?php
enum Test: string
{
    public const ENUM_VALUE = "forty two";

    case TheAnswer = self::ENUM_VALUE;
}
$a = Test::TheAnswer->value;

<?php
/**
 * @psalm-inheritors FirstChoice|SecondChoice|ThirdChoice
 */
interface Choice
{
}

final class FirstChoice implements Choice {}
final class SecondChoice implements Choice {}
final class ThirdChoice implements Choice {}

/**
 * @psalm-assert-if-true FirstChoice $choice
 */
function isFirstChoice(Choice $choice): bool
{
    return $choice instanceof FirstChoice;
}

/**
 * @psalm-assert-if-true SecondChoice $choice
 */
function isSecondChoice(Choice $choice): bool
{
    return $choice instanceof SecondChoice;
}

function testFirstChoice(FirstChoice $_first): string
{
    return "first";
}

function testSecondOrThirdChoice(SecondChoice|ThirdChoice $_first): string
{
    return "second or third";
}

function getLabel(Choice $choice): string
{
    return match (true) {
        isFirstChoice($choice) => testFirstChoice($choice),
        default => testSecondOrThirdChoice($choice),
    };
}

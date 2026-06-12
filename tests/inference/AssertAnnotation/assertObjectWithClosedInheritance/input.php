<?php
/**
 * @psalm-inheritors FirstChoice|SecondChoice|ThirdChoice
 */
interface Choice
{
}

final class FirstChoice implements Choice
{
}

final class SecondChoice implements Choice
{
}

final class ThirdChoice implements Choice
{
}

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

function testFirstChoice(Choice $choice): void
{
    if (isFirstChoice($choice)) {
        /** @psalm-check-type-exact $choice = FirstChoice */
    } else {
        /** @psalm-check-type-exact $choice = SecondChoice|ThirdChoice */
    }
}

function testFirstAndSecondChoice(Choice $choice): void
{
    if (isFirstChoice($choice)) {
        /** @psalm-check-type-exact $choice = FirstChoice */
    } elseif (isSecondChoice($choice)) {
        /** @psalm-check-type-exact $choice = SecondChoice */
    } else {
        /** @psalm-check-type-exact $choice = ThirdChoice */
    }
}

<?php
/**
 * @template T of string
 */
final class App
{
    /**
     * @psalm-if-this-is self<"idle">
     * @psalm-this-out self<"started">
     */
    public function start(): void
    {
        throw new RuntimeException("???");
    }
}

/** @var App<"idle"> */
$app = new App();
$app->start();

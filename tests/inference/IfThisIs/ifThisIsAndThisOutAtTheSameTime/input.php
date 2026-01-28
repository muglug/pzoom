<?php
/**
 * @template T of string
 */
final class App
{
    /**
     * @psalm-if-this-is App<"idle">
     * @psalm-this-out App<"started">
     */
    public function start(): void
    {
        throw new RuntimeException("???");
    }
}

/** @var App<"idle"> */
$app = new App();
$app->start();

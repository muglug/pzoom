<?php
class App {}

function test(App $app) : void {
    if ($app || rand(0, 1)) {}
}

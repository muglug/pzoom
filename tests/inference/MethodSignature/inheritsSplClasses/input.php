<?php
namespace App;

use SplObserver;
use SplSubject;

class Observer implements \SplObserver
{
    public function update(SplSubject $subject): void
    {
    }
}

class Subject implements \SplSubject
{
    public function attach(SplObserver $observer): void
    {
    }

    public function detach(SplObserver $observer): void
    {
    }

    public function notify(): void
    {
    }
}

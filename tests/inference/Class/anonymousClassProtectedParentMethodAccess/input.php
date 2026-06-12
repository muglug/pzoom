<?php

abstract class Base
{
    protected function exporter(): string
    {
        return 'x';
    }
}

function f(): Base
{
    return new class extends Base
    {
        private string $inner = 'a';

        public function go(): string
        {
            return $this->inner . $this->exporter();
        }
    };
}

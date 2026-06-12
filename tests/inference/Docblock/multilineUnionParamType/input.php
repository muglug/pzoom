<?php
interface LegacyIface {}
interface NewIface {}
class ImplNew implements NewIface {}

class Registry {
    /**
     * @param class-string<LegacyIface>
     *     |class-string<NewIface> $class
     */
    public function registerClass(string $class): void {}
}

function reg(Registry $r): void {
    $r->registerClass(ImplNew::class);
}

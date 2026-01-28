<?php
                    namespace A {
                        class Foo {
                            /**
                             * @psalm-internal B\Bar
                             */
                            public static function barBar(): void {
                            }
                        }
                    }

                    namespace B {
                        class Bat {
                            public function batBat() : void {
                                \A\Foo::barBar();
                            }
                        }
                    }

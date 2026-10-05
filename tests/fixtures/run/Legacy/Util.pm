package Legacy::Util;
use strict;
use warnings;

sub new { my ($class) = @_; return bless {}, $class; }
sub name { return "legacy"; }
sub number { return 42; }
sub flag { return 1; }
sub not_a_number { return "abc"; }
sub make_child { return bless {}, 'Legacy::Util::Child'; }
sub emit { my @args = @_; print join(" ", @args), "\n"; return; }

package Legacy::Util::Child;
our @ISA = ('Legacy::Util');

1;

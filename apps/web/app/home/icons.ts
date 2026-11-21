import {
  BellRing,
  Calculator,
  CalendarClock,
  CreditCard,
  History,
  MessageCircle,
  Package,
  ShieldCheck,
  ShoppingCart,
  Wheat,
  type LucideIcon,
} from "lucide-react";

/**
 * Icon keys live in the i18n dictionaries, so a section cannot name an icon
 * that does not exist. One library for the whole page, `currentColor` only.
 */
export const HOME_ICONS: Record<string, LucideIcon> = {
  batches: Package,
  hpp: Calculator,
  recipe: History,
  planning: CalendarClock,
  alert: MessageCircle,
  pos: ShoppingCart,
  shield: ShieldCheck,
  card: CreditCard,
  bell: BellRing,
};

export { Wheat };

/* data.js: parsing, validation and calculation logic with no DOM */

// Parse and validate currency code
export function parseCurrency(text) {
  const trimmed = String(text || '').trim().toUpperCase();
  if (!/^[A-Z]{3}$/.test(trimmed)) {
    return { error: 'Enter a 3-letter currency code like USD' };
  }
  return { value: trimmed };
}

// Parse and validate person name
export function parseName(text, existingNames = []) {
  const trimmed = String(text || '').trim();
  if (trimmed.length < 1 || trimmed.length > 50) {
    return { error: 'Enter a unique name, 1 to 50 characters' };
  }
  if (existingNames.some(n => n.toLowerCase() === trimmed.toLowerCase())) {
    return { error: 'This name is already in the list' };
  }
  return { value: trimmed };
}

// Parse and validate expense description
export function parseDescription(text) {
  const trimmed = String(text || '').trim();
  if (trimmed.length < 1 || trimmed.length > 100) {
    return { error: 'Enter 1 to 100 characters' };
  }
  return { value: trimmed };
}

// Parse and validate amount
export function parseAmount(value) {
  const num = Number(value);
  if (isNaN(num) || num < 0.01 || num > 1000000) {
    return { error: 'Enter an amount from 0.01 to 1,000,000' };
  }
  return { value: num };
}

// Validate split (at least one person selected)
export function validateSplit(personIds) {
  if (!Array.isArray(personIds) || personIds.length === 0) {
    return { error: 'Select at least one person' };
  }
  return { value: personIds };
}

// Calculate each person's share of an expense
// Divides equally, distributing remainder cents to first N people
export function calculateShares(amount, personIds) {
  const count = personIds.length;
  const baseShare = Math.floor((amount * 100) / count) / 100;
  const remainder = Math.round(amount * 100) - Math.round(baseShare * 100) * count;
  
  const shares = {};
  personIds.forEach((id, index) => {
    shares[id] = baseShare + (index < remainder ? 0.01 : 0);
  });
  return shares;
}

// Calculate balances for all people
// Returns { personId: balance } where positive means owed, negative means owes
export function calculateBalances(people, expenses) {
  const balances = {};
  people.forEach(p => balances[p.id] = 0);
  
  expenses.forEach(expense => {
    const shares = calculateShares(expense.amount, expense.splitBetween);
    
    // Each person in the split gets debited their share
    expense.splitBetween.forEach(personId => {
      balances[personId] -= shares[personId];
    });
    
    // Person who paid gets credited the full amount
    balances[expense.paidBy] += expense.amount;
  });
  
  // Round to avoid floating point errors
  Object.keys(balances).forEach(id => {
    balances[id] = Math.round(balances[id] * 100) / 100;
  });
  
  return balances;
}

// Calculate settlement plan: minimum payments to settle all balances
// Returns array of { from: personId, to: personId, amount }
export function calculateSettlement(people, balances) {
  // Create arrays of debtors and creditors with their amounts
  const debts = [];
  const credits = [];
  
  Object.entries(balances).forEach(([personId, balance]) => {
    const id = Number(personId);  // Convert string key back to number
    if (balance < -0.005) {  // owes money (negative balance)
      debts.push({ personId: id, amount: -balance });
    } else if (balance > 0.005) {  // is owed money (positive balance)
      credits.push({ personId: id, amount: balance });
    }
  });
  
  // Sort both by amount descending (largest first)
  debts.sort((a, b) => b.amount - a.amount);
  credits.sort((a, b) => b.amount - a.amount);
  
  const payments = [];
  let debtIndex = 0;
  let creditIndex = 0;
  
  // Greedy algorithm: match largest debtor with largest creditor
  while (debtIndex < debts.length && creditIndex < credits.length) {
    const debtor = debts[debtIndex];
    const creditor = credits[creditIndex];
    
    const amount = Math.min(debtor.amount, creditor.amount);
    
    if (amount > 0.005) {  // Only record if meaningful amount
      payments.push({
        from: debtor.personId,
        to: creditor.personId,
        amount: Math.round(amount * 100) / 100
      });
    }
    
    debtor.amount -= amount;
    creditor.amount -= amount;
    
    if (debtor.amount < 0.005) debtIndex++;
    if (creditor.amount < 0.005) creditIndex++;
  }
  
  return payments;
}

// Get details for one person: what they paid and what they owe
export function getPersonDetails(personId, people, expenses) {
  const paid = [];
  const owes = [];
  
  expenses.forEach(expense => {
    // Did they pay for this?
    if (expense.paidBy === personId) {
      paid.push({
        description: expense.description,
        amount: expense.amount
      });
    }
    
    // Are they in the split, and did someone else pay?
    if (expense.splitBetween.includes(personId) && expense.paidBy !== personId) {
      const shares = calculateShares(expense.amount, expense.splitBetween);
      const payer = people.find(p => p.id === expense.paidBy);
      owes.push({
        description: expense.description,
        paidBy: payer ? payer.name : 'Unknown',
        share: shares[personId]
      });
    }
  });
  
  const totalPaid = paid.reduce((sum, item) => sum + item.amount, 0);
  const totalOwes = owes.reduce((sum, item) => sum + item.share, 0);
  
  return {
    paid,
    owes,
    totalPaid: Math.round(totalPaid * 100) / 100,
    totalOwes: Math.round(totalOwes * 100) / 100
  };
}

// Format amount with currency
export function formatAmount(amount, currency) {
  const absAmount = Math.abs(amount);
  return `${absAmount.toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} ${currency}`;
}

// Generate Markdown export
export function generateMarkdown(people, expenses, currency, balances, settlement) {
  let md = '# Trip Settlement\n\n';
  md += `**Currency:** ${currency}\n\n`;
  
  md += '## Balances\n';
  people.forEach(person => {
    const balance = balances[person.id] || 0;
    const sign = balance >= 0 ? '+' : '−';
    md += `- ${person.name}: ${sign}${formatAmount(balance, currency)}\n`;
  });
  
  md += '\n## Settlement Plan\n';
  if (settlement.length === 0) {
    md += 'All settled up!\n';
  } else {
    settlement.forEach((payment, index) => {
      const from = people.find(p => p.id === payment.from);
      const to = people.find(p => p.id === payment.to);
      md += `${index + 1}. ${from?.name} pays ${to?.name} ${formatAmount(payment.amount, currency)}\n`;
    });
  }
  
  return md;
}

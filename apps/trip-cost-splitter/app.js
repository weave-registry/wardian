/* Trip Cost Splitter: main application logic */
import * as data from './data.js';

// State
let state = {
  currency: 'USD',
  people: [],
  expenses: [],
  nextPersonId: 1,
  nextExpenseId: 1,
  isSampleData: true
};

// Storage helpers - page apps can't use localStorage, so we just keep in memory
// Data is lost on refresh, but sample data loads by default
function loadState() {
  // Load sample data by default
  loadSampleData();
}

function loadSampleData() {
  state.people = [
    { id: 1, name: 'Alice' },
    { id: 2, name: 'Bob' },
    { id: 3, name: 'Charlie' }
  ];
  state.expenses = [
    { id: 1, description: 'Hotel (3 nights)', amount: 450, paidBy: 1, splitBetween: [1, 2, 3] },
    { id: 2, description: 'Groceries', amount: 87.50, paidBy: 2, splitBetween: [1, 2, 3] },
    { id: 3, description: 'Gas', amount: 65, paidBy: 3, splitBetween: [1, 3] }
  ];
  state.nextPersonId = 4;
  state.nextExpenseId = 4;
  state.isSampleData = true;
}

function saveState() {
  // No persistent storage available in sandboxed page apps
  // Users can export and re-import if they need to save data
}

// UI Elements
const currencyInput = document.getElementById('currency');
const currencyHint = document.getElementById('currency-hint');
const amountCurrency = document.getElementById('amountCurrency');
const peopleList = document.getElementById('peopleList');
const personNameInput = document.getElementById('personName');
const personError = document.getElementById('person-error');
const addPersonBtn = document.getElementById('addPersonBtn');
const sampleBadge = document.getElementById('sampleBadge');
const clearSampleBtn = document.getElementById('clearSampleBtn');
const addExpenseBtn = document.getElementById('addExpenseBtn');
const expensesEmpty = document.getElementById('expensesEmpty');
const expensesList = document.getElementById('expensesList');
const balancesGrid = document.getElementById('balancesGrid');
const settlementDiv = document.getElementById('settlement');
const personTabs = document.getElementById('personTabs');
const tabsList = document.getElementById('tabsList');
const tabsContent = document.getElementById('tabsContent');
const exportBtn = document.getElementById('exportBtn');
const expenseDialog = document.getElementById('expenseDialog');
const expenseForm = document.getElementById('expenseForm');
const expenseDescInput = document.getElementById('expenseDesc');
const descError = document.getElementById('desc-error');
const expenseAmountInput = document.getElementById('expenseAmount');
const amountError = document.getElementById('amount-error');
const expensePaidBySelect = document.getElementById('expensePaidBy');
const splitBetweenDiv = document.getElementById('splitBetween');
const splitError = document.getElementById('split-error');
const cancelBtn = document.getElementById('cancelBtn');

// Validation state for fields
const fieldValidation = {
  currency: { dirty: false },
  personName: { dirty: false },
  expenseDesc: { dirty: false },
  expenseAmount: { dirty: false }
};

// Currency field
currencyInput.addEventListener('blur', () => {
  fieldValidation.currency.dirty = true;
  validateCurrency();
});

currencyInput.addEventListener('input', () => {
  if (fieldValidation.currency.dirty) {
    validateCurrency();
  }
  // Check if we can unlock the field
  if (state.expenses.length === 0) {
    currencyInput.disabled = false;
    updateCurrencyHint();
  }
});

function validateCurrency() {
  const result = data.parseCurrency(currencyInput.value);
  if (result.error) {
    currencyHint.textContent = result.error;
    currencyInput.setAttribute('aria-invalid', 'true');
    return false;
  }
  currencyHint.textContent = state.expenses.length > 0 ? 'Clear expenses to change currency' : '';
  currencyInput.removeAttribute('aria-invalid');
  return true;
}

function updateCurrencyHint() {
  if (state.expenses.length > 0) {
    currencyHint.textContent = 'Clear expenses to change currency';
  } else {
    currencyHint.textContent = '';
  }
}

function applyCurrency() {
  if (validateCurrency()) {
    const result = data.parseCurrency(currencyInput.value);
    state.currency = result.value;
    currencyInput.value = state.currency;
    amountCurrency.textContent = state.currency;
    saveState();
    render();
  }
}

currencyInput.addEventListener('change', applyCurrency);

// Add person
personNameInput.addEventListener('blur', () => {
  fieldValidation.personName.dirty = true;
  validatePersonName();
});

personNameInput.addEventListener('input', () => {
  if (fieldValidation.personName.dirty) {
    validatePersonName();
  }
});

function validatePersonName() {
  const existingNames = state.people.map(p => p.name);
  const result = data.parseName(personNameInput.value, existingNames);
  if (result.error) {
    personError.textContent = result.error;
    personNameInput.setAttribute('aria-invalid', 'true');
    return false;
  }
  personError.textContent = '';
  personNameInput.removeAttribute('aria-invalid');
  return true;
}

addPersonBtn.addEventListener('click', () => {
  fieldValidation.personName.dirty = true;
  if (!validatePersonName()) {
    personNameInput.focus();
    return;
  }
  
  const result = data.parseName(personNameInput.value, state.people.map(p => p.name));
  state.people.push({ id: state.nextPersonId++, name: result.value });
  state.isSampleData = false;
  saveState();
  
  personNameInput.value = '';
  fieldValidation.personName.dirty = false;
  personError.textContent = '';
  personNameInput.removeAttribute('aria-invalid');
  
  render();
  personNameInput.focus();
});

personNameInput.addEventListener('keypress', (e) => {
  if (e.key === 'Enter') {
    e.preventDefault();
    addPersonBtn.click();
  }
});

// Remove person
function removePerson(id) {
  const person = state.people.find(p => p.id === id);
  if (!person) return;
  
  // Check if person is in any expenses
  const inExpenses = state.expenses.filter(e => 
    e.paidBy === id || e.splitBetween.includes(id)
  );
  
  if (inExpenses.length > 0) {
    // This shouldn't happen as button should be disabled
    return;
  }
  
  state.people = state.people.filter(p => p.id !== id);
  state.isSampleData = false;
  saveState();
  render();
}

// Clear sample data
clearSampleBtn.addEventListener('click', () => {
  state.people = [];
  state.expenses = [];
  state.nextPersonId = 1;
  state.nextExpenseId = 1;
  state.isSampleData = false;
  saveState();
  render();
});

// Add expense dialog
addExpenseBtn.addEventListener('click', () => {
  if (state.people.length < 2) return;
  
  // Update currency display in dialog
  amountCurrency.textContent = state.currency;
  
  // Populate paid by dropdown
  expensePaidBySelect.innerHTML = state.people
    .map(p => `<option value="${p.id}">${p.name}</option>`)
    .join('');
  
  // Populate split between checkboxes (all checked by default)
  splitBetweenDiv.innerHTML = state.people
    .map(p => `
      <label class="split-checkbox">
        <input type="checkbox" value="${p.id}" checked>
        <span>${p.name}</span>
      </label>
    `)
    .join('');
  
  // Reset form
  expenseForm.reset();
  expensePaidBySelect.value = state.people[0].id;
  fieldValidation.expenseDesc.dirty = false;
  fieldValidation.expenseAmount.dirty = false;
  descError.textContent = '';
  amountError.textContent = '';
  splitError.textContent = '';
  expenseDescInput.removeAttribute('aria-invalid');
  expenseAmountInput.removeAttribute('aria-invalid');
  
  expenseDialog.showModal();
  expenseDescInput.focus();
});

cancelBtn.addEventListener('click', () => {
  expenseDialog.close();
});

// Expense form validation
expenseDescInput.addEventListener('blur', () => {
  fieldValidation.expenseDesc.dirty = true;
  validateExpenseDesc();
});

expenseDescInput.addEventListener('input', () => {
  if (fieldValidation.expenseDesc.dirty) {
    validateExpenseDesc();
  }
});

function validateExpenseDesc() {
  const result = data.parseDescription(expenseDescInput.value);
  if (result.error) {
    descError.textContent = result.error;
    expenseDescInput.setAttribute('aria-invalid', 'true');
    return false;
  }
  descError.textContent = '';
  expenseDescInput.removeAttribute('aria-invalid');
  return true;
}

expenseAmountInput.addEventListener('blur', () => {
  fieldValidation.expenseAmount.dirty = true;
  validateExpenseAmount();
});

expenseAmountInput.addEventListener('input', () => {
  if (fieldValidation.expenseAmount.dirty) {
    validateExpenseAmount();
  }
});

function validateExpenseAmount() {
  const result = data.parseAmount(expenseAmountInput.value);
  if (result.error) {
    amountError.textContent = result.error;
    expenseAmountInput.setAttribute('aria-invalid', 'true');
    return false;
  }
  amountError.textContent = '';
  expenseAmountInput.removeAttribute('aria-invalid');
  return true;
}

function validateExpenseSplit() {
  const checkboxes = splitBetweenDiv.querySelectorAll('input[type="checkbox"]:checked');
  const personIds = Array.from(checkboxes).map(cb => Number(cb.value));
  const result = data.validateSplit(personIds);
  if (result.error) {
    splitError.textContent = result.error;
    return false;
  }
  splitError.textContent = '';
  return true;
}

// Add expense form submit
expenseForm.addEventListener('submit', (e) => {
  e.preventDefault();
  
  fieldValidation.expenseDesc.dirty = true;
  fieldValidation.expenseAmount.dirty = true;
  
  const descValid = validateExpenseDesc();
  const amountValid = validateExpenseAmount();
  const splitValid = validateExpenseSplit();
  
  if (!descValid || !amountValid || !splitValid) {
    if (!descValid) expenseDescInput.focus();
    else if (!amountValid) expenseAmountInput.focus();
    return;
  }
  
  const descResult = data.parseDescription(expenseDescInput.value);
  const amountResult = data.parseAmount(expenseAmountInput.value);
  const checkboxes = splitBetweenDiv.querySelectorAll('input[type="checkbox"]:checked');
  const splitBetween = Array.from(checkboxes).map(cb => Number(cb.value));
  
  state.expenses.push({
    id: state.nextExpenseId++,
    description: descResult.value,
    amount: amountResult.value,
    paidBy: Number(expensePaidBySelect.value),
    splitBetween
  });
  
  state.isSampleData = false;
  
  // Lock currency after first expense
  if (state.expenses.length === 1) {
    currencyInput.disabled = true;
    updateCurrencyHint();
  }
  
  saveState();
  render();
  expenseDialog.close();
});

// Delete expense
function deleteExpense(id) {
  state.expenses = state.expenses.filter(e => e.id !== id);
  state.isSampleData = false;
  
  // Unlock currency if no expenses
  if (state.expenses.length === 0) {
    currencyInput.disabled = false;
    updateCurrencyHint();
  }
  
  saveState();
  render();
}

// Export
exportBtn.addEventListener('click', async () => {
  const balances = data.calculateBalances(state.people, state.expenses);
  const settlement = data.calculateSettlement(state.people, balances);
  const markdown = data.generateMarkdown(state.people, state.expenses, state.currency, balances, settlement);
  
  try {
    await navigator.clipboard.writeText(markdown);
    WardianUI.toast('Copied to clipboard!', { variant: 'success' });
  } catch (e) {
    console.error('Failed to copy:', e);
    WardianUI.toast('Failed to copy to clipboard', { variant: 'destructive' });
  }
});

// Render functions
function render() {
  renderPeople();
  renderExpenses();
  renderBalances();
  renderSettlement();
  renderPersonTabs();
  updateSampleBadge();
  updateAddExpenseButton();
}

function renderPeople() {
  if (state.people.length === 0) {
    peopleList.innerHTML = '<li class="expenses-empty">Add the first person to get started</li>';
    return;
  }
  
  peopleList.innerHTML = state.people.map(person => {
    const inExpenses = state.expenses.filter(e => 
      e.paidBy === person.id || e.splitBetween.includes(person.id)
    );
    const canRemove = inExpenses.length === 0;
    const tooltip = canRemove ? '' : `data-tooltip="Cannot remove ${person.name}: appears in ${inExpenses.length} expense(s)"`;
    
    return `
      <li class="person-item">
        <span class="person-name">${person.name}</span>
        <button 
          class="w-button" 
          data-variant="ghost" 
          data-size="icon" 
          aria-label="Remove ${person.name}"
          ${canRemove ? '' : 'disabled'}
          ${tooltip}
          onclick="window.removePerson(${person.id})"
        >✕</button>
      </li>
    `;
  }).join('');
}

function renderExpenses() {
  if (state.people.length < 2) {
    expensesEmpty.textContent = 'Add at least 2 people to record expenses';
    expensesEmpty.hidden = false;
    expensesList.hidden = true;
    return;
  }
  
  if (state.expenses.length === 0) {
    expensesEmpty.textContent = 'No expenses yet. Add one to get started';
    expensesEmpty.hidden = false;
    expensesList.hidden = true;
    return;
  }
  
  expensesEmpty.hidden = true;
  expensesList.hidden = false;
  
  // Newest first
  const sorted = [...state.expenses].reverse();
  
  expensesList.innerHTML = sorted.map(expense => {
    const payer = state.people.find(p => p.id === expense.paidBy);
    const splitNames = expense.splitBetween
      .map(id => state.people.find(p => p.id === id)?.name)
      .filter(Boolean)
      .join(', ');
    
    return `
      <li class="expense-item">
        <div class="expense-header">
          <div>
            <div class="expense-desc">${expense.description}</div>
            <div class="expense-amount">${data.formatAmount(expense.amount, state.currency)}</div>
          </div>
          <button 
            class="w-button" 
            data-variant="ghost" 
            data-size="icon" 
            aria-label="Delete ${expense.description}"
            onclick="window.deleteExpense(${expense.id})"
          >✕</button>
        </div>
        <div class="expense-meta">
          Paid by ${payer?.name || 'Unknown'}<br>
          Split between ${splitNames}
        </div>
      </li>
    `;
  }).join('');
}

function renderBalances() {
  if (state.people.length === 0 || state.expenses.length === 0) {
    balancesGrid.innerHTML = '<div class="expenses-empty">Add expenses to see balances</div>';
    return;
  }
  
  const balances = data.calculateBalances(state.people, state.expenses);
  
  // Sort by balance descending (most owed first)
  const sorted = [...state.people].sort((a, b) => 
    (balances[b.id] || 0) - (balances[a.id] || 0)
  );
  
  balancesGrid.innerHTML = sorted.map(person => {
    const balance = balances[person.id] || 0;
    // Half a cent: balances are rounded to 2 decimals
    const sign = balance > 0.005 ? '+' : (balance < -0.005 ? '−' : '');
    let colorClass = 'balance-zero';
    if (balance > 0.005) colorClass = 'balance-positive';
    else if (balance < -0.005) colorClass = 'balance-negative';
    
    return `
      <div class="balance-item">
        <div class="balance-name">${person.name}</div>
        <div class="balance-amount ${colorClass}">
          ${sign}${data.formatAmount(balance, state.currency)}
        </div>
      </div>
    `;
  }).join('');
}

function renderSettlement() {
  if (state.people.length === 0 || state.expenses.length === 0) {
    settlementDiv.innerHTML = '<div class="settlement-empty">All settled up!</div>';
    return;
  }
  
  const balances = data.calculateBalances(state.people, state.expenses);
  const settlement = data.calculateSettlement(state.people, balances);
  
  if (settlement.length === 0) {
    settlementDiv.innerHTML = '<div class="settlement-empty">All settled up!</div>';
    return;
  }
  
  settlementDiv.innerHTML = `
    <ol class="settlement-list">
      ${settlement.map(payment => {
        const from = state.people.find(p => p.id === payment.from);
        const to = state.people.find(p => p.id === payment.to);
        return `
          <li class="settlement-item">
            ${from?.name} pays ${to?.name} ${data.formatAmount(payment.amount, state.currency)}
          </li>
        `;
      }).join('')}
    </ol>
  `;
}

function renderPersonTabs() {
  if (state.people.length === 0) {
    personTabs.style.display = 'none';
    return;
  }
  
  personTabs.style.display = 'block';
  
  // Render tab buttons
  tabsList.innerHTML = state.people.map((person, index) => `
    <button role="tab" aria-controls="tab-${person.id}" ${index === 0 ? 'aria-selected="true"' : ''}>
      ${person.name}
    </button>
  `).join('');
  
  // Render tab panels
  tabsContent.innerHTML = state.people.map((person, index) => {
    const details = data.getPersonDetails(person.id, state.people, state.expenses);
    
    return `
      <div role="tabpanel" id="tab-${person.id}" ${index !== 0 ? 'hidden' : ''}>
        <div class="details-section">
          <h3 class="details-heading">Paid</h3>
          ${details.paid.length === 0 ? 
            '<div class="details-empty">Hasn\'t paid for anything yet</div>' :
            `
              <ul class="details-list">
                ${details.paid.map(item => `
                  <li class="details-item">
                    <span class="details-desc">${item.description}</span>
                    <span class="details-amount">${data.formatAmount(item.amount, state.currency)}</span>
                  </li>
                `).join('')}
              </ul>
              <div class="details-total">
                <span>Total paid</span>
                <span>${data.formatAmount(details.totalPaid, state.currency)}</span>
              </div>
            `
          }
        </div>
        
        <div class="details-section">
          <h3 class="details-heading">Owes</h3>
          ${details.owes.length === 0 ?
            '<div class="details-empty">Doesn\'t owe anything</div>' :
            `
              <ul class="details-list">
                ${details.owes.map(item => `
                  <li class="details-item">
                    <div>
                      <div class="details-desc">${item.description}</div>
                      <div class="details-meta">Paid by ${item.paidBy}</div>
                    </div>
                    <span class="details-amount">${data.formatAmount(item.share, state.currency)}</span>
                  </li>
                `).join('')}
              </ul>
              <div class="details-total">
                <span>Total owes</span>
                <span>${data.formatAmount(details.totalOwes, state.currency)}</span>
              </div>
            `
          }
        </div>
      </div>
    `;
  }).join('');
}

function updateSampleBadge() {
  sampleBadge.hidden = !state.isSampleData;
}

function updateAddExpenseButton() {
  if (state.people.length < 2) {
    addExpenseBtn.disabled = true;
    addExpenseBtn.setAttribute('data-tooltip', 'Add at least 2 people first');
  } else {
    addExpenseBtn.disabled = false;
    addExpenseBtn.removeAttribute('data-tooltip');
  }
}

// Export functions to window for onclick handlers
window.removePerson = removePerson;
window.deleteExpense = deleteExpense;

// Initialize
loadState();
updateCurrencyHint();
render();
